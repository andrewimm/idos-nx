pub mod term;
pub mod textmode;

use self::term::Terminal;
use alloc::{collections::VecDeque, vec::Vec};

pub struct Console<const COLS: usize, const ROWS: usize> {
    pub terminal: Terminal<COLS, ROWS>,

    /// Stores input that has been entered but not yet flushed to a reader
    pending_input: Vec<u8>,
    /// Stores flushed input. The next read operation on this console will
    /// pull bytes from this.
    pub flushed_input: VecDeque<u8>,
}

impl<const COLS: usize, const ROWS: usize> Console<COLS, ROWS> {
    pub fn new() -> Self {
        let terminal = Terminal::new();
        Self {
            terminal,
            pending_input: Vec::new(),
            flushed_input: VecDeque::new(),
        }
    }

    /// Send bytes of input from the keyboard. If input is flushed, all pending
    /// input will be moved to the flushed input buffer.
    pub fn send_input(&mut self, input: &[u8]) {
        let mut should_flush = false;
        for ch in input {
            if *ch == 0x0a {
                should_flush = true;
            } else if *ch == 0x08 {
                if !self.pending_input.is_empty() {
                    self.terminal.backspace();
                    self.terminal.write_character(0x20); // Write a space to clear the character
                    self.pending_input.pop();
                    self.terminal.backspace(); // Move cursor back again, since writing space moved it forward
                }
                continue;
            }
            self.terminal.write_character(*ch);

            self.pending_input.push(*ch);
        }

        if should_flush {
            for byte in self.pending_input.iter() {
                self.flushed_input.push_back(*byte);
            }
            self.pending_input.clear();
        }
    }

    pub fn send_output(&mut self, output: &[u8]) {
        for ch in output {
            self.terminal.write_character(*ch);
        }
    }

    /// Construct an iterator over the text glyphs in a specific row of the
    /// screen.
    pub fn row_text_iter(
        &self,
        row: usize,
    ) -> core::iter::StepBy<core::iter::Cloned<core::slice::Iter<'_, u8>>> {
        let row_size = COLS * core::mem::size_of::<textmode::TextCell>();
        let offset = row * COLS * core::mem::size_of::<textmode::TextCell>();

        let buffer = unsafe {
            let ptr: *mut u8 = self
                .terminal
                .text_buffer
                .get_visible_buffer_byte_ptr()
                .add(offset);
            core::slice::from_raw_parts_mut(ptr, row_size)
        };

        buffer.iter().cloned().step_by(2)
    }
}
