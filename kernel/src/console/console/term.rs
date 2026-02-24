use alloc::boxed::Box;

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
    current_color: ColorCode,
    parse_state: AnsiParseState,
    ansi_params: Vec<u8>,

    pub text_buffer: TextBuffer<COLS, ROWS>,

    pub graphics_buffer: Option<GraphicsBuffer>,

    /// Custom 256-color palette stored as 0x00RRGGBB. When None, the default
    /// VGA palette is used for color lookups in both text and graphics mode.
    pub palette: Option<Box<[u32; 256]>>,

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
    /// Returns the full buffer including the 8-byte dirty rect header.
    pub fn get_buffer(&self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.vaddr.as_ptr_mut(), self.allocated_size) }
    }

    /// Returns just the pixel data (after the 8-byte header).
    pub fn get_pixels(&self) -> &mut [u8] {
        &mut self.get_buffer()[8..]
    }

    /// Read the dirty rect header as (x, y, w, h). Returns None if all zeros (clean).
    pub fn read_dirty_rect(&self) -> Option<(u16, u16, u16, u16)> {
        let buf = self.get_buffer();
        let x = u16::from_le_bytes([buf[0], buf[1]]);
        let y = u16::from_le_bytes([buf[2], buf[3]]);
        let w = u16::from_le_bytes([buf[4], buf[5]]);
        let h = u16::from_le_bytes([buf[6], buf[7]]);
        if w == 0 && h == 0 {
            None
        } else {
            Some((x, y, w, h))
        }
    }

    /// Clear the dirty rect header (mark as clean).
    pub fn clear_dirty_rect(&self) {
        let buf = self.get_buffer();
        buf[0..8].fill(0);
    }
}

impl<const COLS: usize, const ROWS: usize> Terminal<COLS, ROWS> {
    pub fn new() -> Self {
        let alloc_buffer = crate::task::actions::memory::map_memory(
            None,
            0x2000,
            crate::task::memory::MemoryBacking::FreeMemory,
        )
        .unwrap();
        let text_buffer = TextBuffer::new(alloc_buffer, 0x2000);

        Self {
            cursor_x: 0,
            cursor_y: (ROWS - 2) as u8,
            current_color: ColorCode::new(Color::LightGray, Color::Black),
            parse_state: AnsiParseState::Normal,
            ansi_params: Vec::new(),

            text_buffer,
            graphics_buffer: None,
            palette: None,

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
                    self.ansi_params.clear();
                    self.parse_state = AnsiParseState::CSI;
                } else {
                    // Other escape sequences aren't handled
                    self.parse_state = AnsiParseState::Normal;
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
        let cell = &mut self.text_buffer.get_text_buffer()[absolute_offset];
        cell.glyph = ch;
        cell.color = self.current_color;

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

    /// Parse the collected CSI parameter bytes into a list of numeric values.
    /// Parameters are separated by ';'. Missing or empty params default to 0.
    fn parse_csi_params(&self) -> Vec<u32> {
        let mut params = Vec::new();
        let mut current: u32 = 0;
        let mut has_digit = false;
        for &byte in &self.ansi_params {
            if byte == b';' {
                params.push(if has_digit { current } else { 0 });
                current = 0;
                has_digit = false;
            } else if byte >= b'0' && byte <= b'9' {
                current = current * 10 + (byte - b'0') as u32;
                has_digit = true;
            }
        }
        params.push(if has_digit { current } else { 0 });
        params
    }

    pub fn handle_csi(&mut self, final_char: u8) {
        let params = self.parse_csi_params();
        self.ansi_params.clear();

        match final_char {
            b'A' => {
                // Cursor up
                let n = params.first().copied().unwrap_or(1).max(1) as u8;
                self.cursor_y = self.cursor_y.saturating_sub(n);
            }
            b'B' => {
                // Cursor down
                let n = params.first().copied().unwrap_or(1).max(1) as u8;
                self.cursor_y = (self.cursor_y + n).min((ROWS - 1) as u8);
            }
            b'C' => {
                // Cursor forward
                let n = params.first().copied().unwrap_or(1).max(1) as u8;
                self.cursor_x = (self.cursor_x + n).min((COLS - 1) as u8);
            }
            b'D' => {
                // Cursor back
                let n = params.first().copied().unwrap_or(1).max(1) as u8;
                self.cursor_x = self.cursor_x.saturating_sub(n);
            }
            b'H' | b'f' => {
                // Cursor position: ESC[row;colH
                let row = params.first().copied().unwrap_or(1).max(1) - 1;
                let col = params.get(1).copied().unwrap_or(1).max(1) - 1;
                self.cursor_y = (row as u8).min((ROWS - 1) as u8);
                self.cursor_x = (col as u8).min((COLS - 1) as u8);
            }
            b'J' => {
                // Erase in display
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    2 => {
                        // Clear entire screen
                        self.clear_buffer();
                        self.cursor_x = 0;
                        self.cursor_y = 0;
                    }
                    _ => {}
                }
            }
            b'm' => {
                // SGR - Select Graphic Rendition
                // If no params, treat as reset (0)
                let params = if params.is_empty() || (params.len() == 1 && params[0] == 0) {
                    &[0u32][..]
                } else {
                    &params[..]
                };
                for &code in params {
                    self.handle_sgr(code);
                }
            }
            _ => {}
        }
    }

    fn handle_sgr(&mut self, code: u32) {
        use super::textmode::ansi_color_to_vga;
        match code {
            0 => {
                // Reset
                self.current_color = ColorCode::new(Color::LightGray, Color::Black);
            }
            1 => {
                // Bold / bright: promote fg to bright variant
                let fg = self.current_color.fg();
                if fg < 8 {
                    self.current_color.set_fg(fg + 8);
                }
            }
            22 => {
                // Normal intensity: demote fg to normal variant
                let fg = self.current_color.fg();
                if fg >= 8 {
                    self.current_color.set_fg(fg - 8);
                }
            }
            7 => {
                // Reverse video: swap fg and bg
                let fg = self.current_color.fg();
                let bg = self.current_color.bg();
                self.current_color.set_fg(bg);
                self.current_color.set_bg(fg);
            }
            30..=37 => {
                // Normal foreground colors
                self.current_color.set_fg(ansi_color_to_vga((code - 30) as u8, false));
            }
            39 => {
                // Default foreground
                self.current_color.set_fg(Color::LightGray as u8);
            }
            40..=47 => {
                // Normal background colors
                self.current_color.set_bg(ansi_color_to_vga((code - 40) as u8, false));
            }
            49 => {
                // Default background
                self.current_color.set_bg(Color::Black as u8);
            }
            90..=97 => {
                // Bright foreground colors
                self.current_color.set_fg(ansi_color_to_vga((code - 90) as u8, true));
            }
            100..=107 => {
                // Bright background colors
                self.current_color.set_bg(ansi_color_to_vga((code - 100) as u8, true));
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

    pub fn get_palette(&self) -> &[u32; 256] {
        match &self.palette {
            Some(p) => p,
            None => &crate::console::graphics::palette::VGA_PALETTE,
        }
    }

    /// Copy the active palette out to packed RGB bytes (R, G, B per entry)
    pub fn get_palette_rgb(&self, out: &mut [u8]) {
        let palette = self.get_palette();
        for (i, &color) in palette.iter().enumerate() {
            let offset = i * 3;
            if offset + 2 >= out.len() {
                break;
            }
            out[offset] = (color >> 16) as u8; // R
            out[offset + 1] = (color >> 8) as u8; // G
            out[offset + 2] = color as u8; // B
        }
    }

    /// Load a palette from packed RGB bytes (R, G, B per entry) into internal u32 format
    pub fn set_palette_rgb(&mut self, data: &[u8]) {
        let palette = self.palette.get_or_insert_with(|| {
            Box::new(crate::console::graphics::palette::VGA_PALETTE)
        });
        for i in 0..256 {
            let offset = i * 3;
            if offset + 2 >= data.len() {
                break;
            }
            let r = data[offset] as u32;
            let g = data[offset + 1] as u32;
            let b = data[offset + 2] as u32;
            palette[i] = (r << 16) | (g << 8) | b;
        }
    }

    pub fn set_graphics_mode(&mut self, graphics_struct: &mut termios::GraphicsMode) {
        if let Some(_existing_buffer) = &self.graphics_buffer {
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
            crate::task::memory::MemoryBacking::FreeMemory,
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
