use alloc::{sync::Arc, vec::Vec};

use crate::{collections::RingBuffer, memory::address::VirtualAddress};

use super::buffers::ConsoleBuffers;

pub struct Console {
    cursor_x: u8,
    cursor_y: u8,

    width: u8,
    height: u8,

    text_buffer_base: VirtualAddress,
    text_buffer_offset: usize,
    text_buffer_stride: usize,

    // stores user input that will be flushed on the next newline
    // this is bypassed if the console is in "raw" mode
    pending_input: Vec<u8>,
}

impl Console {
    pub fn new(text_buffer_base: VirtualAddress) -> Self {
        Self {
            cursor_x: 0,
            cursor_y: 22,

            width: 80,
            height: 24,

            text_buffer_base,
            text_buffer_offset: 80,
            text_buffer_stride: 80,

            pending_input: Vec::new(),
        }
    }

    pub fn clear_buffer(&self) {
        let buffer = self.get_text_buffer();
        for cell in buffer.iter_mut() {
            cell.glyph = 0x20;
            cell.color = ColorCode::new(Color::LightGrey, Color::Black);
        }
    }

    pub fn get_text_buffer_base_ptr(&self) -> *mut TextCell {
        self.text_buffer_base.as_ptr_mut::<TextCell>()
    }

    pub fn get_text_buffer(&self) -> &mut [TextCell] {
        unsafe {
            let ptr = self.get_text_buffer_base_ptr();
            let len = 80 * 25;
            core::slice::from_raw_parts_mut(ptr, len)
        }
    }

    pub fn row_text_iter(
        &self,
        row: usize,
    ) -> core::iter::StepBy<core::iter::Cloned<core::slice::Iter<'_, u8>>> {
        let row_size = self.text_buffer_stride as usize * 2;
        let offset = self.text_buffer_stride * row * 2;

        let buffer = unsafe {
            let ptr = self.text_buffer_base.as_ptr_mut::<u8>().add(offset);
            core::slice::from_raw_parts_mut(ptr, row_size)
        };

        buffer.iter().cloned().step_by(2)
    }

    pub fn get_width(&self) -> u8 {
        self.width
    }

    pub fn get_height(&self) -> u8 {
        self.height
    }

    pub fn send_input(&mut self, input: &[u8]) -> bool {
        let mut should_flush = false;
        for ch in input {
            if *ch == 0x0a {
                should_flush = true;
            } else if *ch == 0x08 {
                if !self.pending_input.is_empty() {
                    // TODO: implement inverse of "advance cursor"
                    self.cursor_x -= 1;

                    self.write_character(0x20);
                    self.pending_input.pop();

                    self.cursor_x -= 1;
                }
                continue;
            }
            self.write_character(*ch);

            self.pending_input.push(*ch);
        }
        should_flush
    }

    pub fn flush_pending_input(&mut self, buffers: &ConsoleBuffers) -> usize {
        for ch in self.pending_input.iter() {
            buffers.input_buffer.write(*ch);
        }
        let len = self.pending_input.len();
        self.pending_input.clear();
        len
    }

    pub fn write_character(&mut self, ch: u8) {
        if ch == 0x0a {
            self.carriage_return();
            self.newline();
        } else if ch < 0x20 {
            // non printable
        } else {
            self.put_raw_character(ch);
        }
    }

    pub fn put_raw_character(&mut self, ch: u8) {
        let absolute_offset = self.get_cursor_offset();
        self.get_text_buffer()[absolute_offset].glyph = ch;

        self.advance_cursor();
    }

    pub fn advance_cursor(&mut self) {
        self.cursor_x += 1;
        if self.cursor_x >= self.width {
            self.cursor_x -= self.width;
            self.newline();
        }
    }

    pub fn get_cursor_offset(&self) -> usize {
        self.text_buffer_offset
            + self.cursor_y as usize * self.text_buffer_stride
            + self.cursor_x as usize
    }

    pub fn carriage_return(&mut self) {
        self.cursor_x = 0;
    }

    pub fn newline(&mut self) {
        self.cursor_y += 1;
        if self.cursor_y >= self.height {
            self.cursor_y = self.height - 1;
        }
        self.scroll(1);
    }

    pub fn scroll(&mut self, scroll_delta: u8) {
        if scroll_delta == 0 {
            return;
        }
        if scroll_delta >= self.height {
            //self.clear_screen();
            return;
        }
        let rows_to_scroll = self.height - scroll_delta;
        for row in 0..rows_to_scroll {
            let to = unsafe {
                let row_offset = self.text_buffer_offset + self.text_buffer_stride * row as usize;
                let to_ptr = self.get_text_buffer_base_ptr().add(row_offset);
                core::slice::from_raw_parts_mut(to_ptr, self.width as usize)
            };
            let from = unsafe {
                let row_offset = self.text_buffer_offset
                    + self.text_buffer_stride * (row + scroll_delta) as usize;
                let from_ptr = self.get_text_buffer_base_ptr().add(row_offset);
                core::slice::from_raw_parts(from_ptr, self.width as usize)
            };
            to.copy_from_slice(from);
        }

        for row in rows_to_scroll..self.height {
            let row_offset = self.text_buffer_offset + self.text_buffer_stride * row as usize;
            let row_end = row_offset + self.width as usize;
            let row_buffer = &mut self.get_text_buffer()[row_offset..row_end];
            for cell in row_buffer {
                cell.glyph = 0x20;
            }
        }
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct TextCell {
    pub glyph: u8,
    pub color: ColorCode,
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct ColorCode(pub u8);

impl ColorCode {
    pub const fn new(fg: Color, bg: Color) -> Self {
        Self((bg as u8) << 4 | (fg as u8))
    }

    pub fn set_fg(&mut self, fg: Color) {
        self.0 &= 0xf0;
        self.0 |= fg as u8;
    }
}

#[derive(Copy, Clone)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue,
    Green,
    Cyan,
    Red,
    Magenta,
    Brown,
    LightGrey,
    DarkGrey,
    LightBlue,
    LightGreen,
    LightCyan,
    LightRed,
    LightMagenta,
    LightBrown,
    White,
}
