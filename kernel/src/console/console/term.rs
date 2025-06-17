use super::textmode::{Color, ColorCode, TextBuffer};

/// Handles terminal state management, including handling of ANSI escape codes
/// and other terminal emulation. The Terminal struct is not responsible for
/// line disposition, that is handled by the top-level Console.
pub struct Terminal<const COLS: usize, const ROWS: usize> {
    cursor_x: u8,
    cursor_y: u8,
    parse_state: AnsiParseState,
    ansi_params: Vec<u8>,

    pub text_buffer: TextBuffer<COLS, ROWS>,
}

impl<const COLS: usize, const ROWS: usize> Terminal<COLS, ROWS> {
    pub fn new() -> Self {
        let alloc_buffer = crate::task::actions::memory::map_memory(
            None,
            0x2000,
            crate::task::memory::MemoryBacking::Anonymous,
        )
        .unwrap();
        let text_buffer = TextBuffer::new::<COLS, ROWS>(alloc_buffer, 0x2000);

        Self {
            cursor_x: 0,
            cursor_y: (ROWS - 2) as u8,
            parse_state: AnsiParseState::Normal,
            ansi_params: Vec::new(),

            text_buffer,
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
            cell.color = ColorCode::new(Color::LightGrey, Color::Black);
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
                    self.parse_state = AnsiParseState::Csi;
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
}

pub enum AnsiParseState {
    Normal,
    Escape,
    CSI,
}
