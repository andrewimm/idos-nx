use crate::{memory::address::VirtualAddress, task::switching::get_current_id};

use super::textmode::{Color, ColorCode, TextBuffer};
use alloc::vec::Vec;
use idos_api::io::termios;

/// Handles terminal state management, including handling of ANSI escape codes
/// and other terminal emulation. The Terminal struct is not responsible for
/// line disposition, that is handled by the top-level Console.
pub struct Terminal<const COLS: usize, const ROWS: usize> {
    cursor_x: u8,
    cursor_y: u8,
    parse_state: AnsiParseState,
    ansi_params: Vec<u8>,

    pub text_buffer: TextBuffer<COLS, ROWS>,

    pub graphics_buffer: Option<GraphicsBuffer>,

    // termios flags
    pub iflags: u32,
    pub oflags: u32,
    pub cflags: u32,
    pub lflags: u32,
}

pub struct GraphicsBuffer {
    pub vaddr: VirtualAddress,
    pub allocated_size: usize,

    pub width: u16,
    pub height: u16,
    pub bits_per_pixel: usize,
}

impl GraphicsBuffer {
    pub fn get_buffer(&self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.vaddr.as_ptr_mut(), self.allocated_size) }
    }
}

impl<const COLS: usize, const ROWS: usize> Terminal<COLS, ROWS> {
    pub fn new() -> Self {
        let alloc_buffer = crate::task::actions::memory::map_memory(
            None,
            0x2000,
            crate::task::memory::MemoryBacking::Anonymous,
        )
        .unwrap();
        let text_buffer = TextBuffer::new(alloc_buffer, 0x2000);

        Self {
            cursor_x: 0,
            cursor_y: (ROWS - 2) as u8,
            parse_state: AnsiParseState::Normal,
            ansi_params: Vec::new(),

            text_buffer,
            graphics_buffer: None,

            iflags: 0,
            oflags: 0,
            cflags: 0,
            lflags: termios::ECHO | termios::ICANON | termios::ISIG,
        }
    }

    pub fn get_cursor_position(&self) -> (u8, u8) {
        (self.cursor_x, self.cursor_y)
    }

    pub fn set_cursor_position(&mut self, x: u8, y: u8) {
        self.cursor_x = x;
        self.cursor_y = y;
    }

    pub fn clear_buffer(&self) {
        let buffer = self.text_buffer.get_text_buffer();
        for cell in buffer.iter_mut() {
            cell.glyph = 0x20; // Space character
            cell.color = ColorCode::new(Color::LightGray, Color::Black);
        }
    }

    /// Send a character to be output to the terminal. Special cases that will
    /// modify terminal state will be handled here.
    pub fn write_character(&mut self, ch: u8) {
        match self.parse_state {
            AnsiParseState::Normal => {
                if ch == 0x0a {
                    self.carriage_return();
                    self.newline();
                } else if ch == 0x1b {
                    // Handle ANSI escape codes
                    self.parse_state = AnsiParseState::Escape;
                } else if ch < 0x20 {
                    // don't output the other non-printable characters
                } else {
                    self.put_raw_character(ch);
                }
            }
            AnsiParseState::Escape => {
                if ch == b'[' {
                    self.parse_state = AnsiParseState::CSI;
                } else {
                    // Other escape sequences aren't handled
                    self.parse_state = AnsiParseState::Normal; // Reset state
                }
            }
            AnsiParseState::CSI => {
                match ch {
                    0x20..=0x2f => {
                        // intermediate bytes are also pushed
                        self.ansi_params.push(ch);
                    }
                    0x30..=0x3f => {
                        // Collect parameters for CSI sequences
                        self.ansi_params.push(ch);
                    }
                    0x40..=0x7e => {
                        // Final byte of the CSI sequence
                        self.handle_csi(ch);
                        self.parse_state = AnsiParseState::Normal;
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn put_raw_character(&mut self, ch: u8) {
        let absolute_offset = self.get_cursor_offset();
        self.text_buffer.get_text_buffer()[absolute_offset].glyph = ch;

        self.advance_cursor();
    }

    pub fn advance_cursor(&mut self) {
        self.cursor_x += 1;
        if self.cursor_x >= COLS as u8 {
            self.cursor_x = 0;
            self.newline();
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        } else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = (COLS - 1) as u8;
        }
    }

    pub fn get_cursor_offset(&self) -> usize {
        self.cursor_y as usize * COLS + self.cursor_x as usize
    }

    /// Move the cursor to the beginning of the line
    pub fn carriage_return(&mut self) {
        self.cursor_x = 0;
    }

    /// Move the cursor down a line. If the cursor is already at the bottom of
    /// the terminal, scroll the content up. This does not move the cursor
    /// horizontally.
    pub fn newline(&mut self) {
        self.cursor_y += 1;
        if self.cursor_y >= ROWS as u8 {
            self.text_buffer.scroll();
            self.cursor_y = (ROWS - 1) as u8; // Keep cursor within bounds
        }
    }

    pub fn handle_csi(&mut self, final_char: u8) {
        match final_char {
            b'A' => {
                // Cursor up
            }
            _ => {}
        }
    }

    pub fn set_termios(&mut self, termios_struct: &termios::Termios) {
        self.iflags = termios_struct.iflags;
        self.oflags = termios_struct.oflags;
        self.cflags = termios_struct.cflags;
        self.lflags = termios_struct.lflags;
    }

    pub fn get_termios(&self, termios_struct: &mut termios::Termios) {
        termios_struct.iflags = self.iflags;
        termios_struct.oflags = self.oflags;
        termios_struct.cflags = self.cflags;
        termios_struct.lflags = self.lflags;
    }

    pub fn set_graphics_mode(&mut self, graphics_struct: &mut termios::GraphicsMode) {
        if let Some(existing_buffer) = &self.graphics_buffer {
            // resize the graphics buffer if necessary, otherwise do nothing
            unimplemented!()
        }
        // color depth is in the first 8 bits, and is a number of bits-per-pixel
        // not all values are valid, we should probably validate here and also
        // have some error handling...
        let bits_per_pixel = graphics_struct.bpp_flags as usize & 0xff;
        let bytes_per_pixel = (bits_per_pixel + 7) / 8;
        // the buffer starts with 4 16-bit values that are used to signal to
        // the compositor that the buffer is dirty. They represent a dirty
        // rectangle in (x, y, width, height) format. To redraw the whole screen
        // set bytes 4-7 to 0xff.
        let buffer_size = 8
            + (graphics_struct.width as usize)
                * (graphics_struct.height as usize)
                * bytes_per_pixel;

        let pages_needed = (buffer_size + 0xfff) / 0x1000;
        let buffer_vaddr = crate::task::actions::memory::map_memory(
            None,
            pages_needed as u32 * 0x1000,
            crate::task::memory::MemoryBacking::Anonymous,
        )
        .unwrap();

        let paddr = crate::task::paging::get_current_physical_address(buffer_vaddr).unwrap();

        self.graphics_buffer = Some(GraphicsBuffer {
            vaddr: buffer_vaddr,
            allocated_size: pages_needed * 0x1000,
            width: graphics_struct.width,
            height: graphics_struct.height,
            bits_per_pixel,
        });

        graphics_struct.framebuffer = paddr.as_u32();
    }

    pub fn exit_graphics_mode(&mut self) {
        if let Some(existing_buffer) = self.graphics_buffer.take() {
            crate::task::actions::memory::unmap_memory_for_task(
                get_current_id(),
                existing_buffer.vaddr,
                existing_buffer.allocated_size as u32,
            )
            .unwrap();
            crate::kprintln!("EXIT GRAPHICS MODE");
        }
    }
}

pub enum AnsiParseState {
    Normal,
    Escape,
    CSI,
}
