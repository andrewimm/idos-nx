pub mod term;
pub mod textmode;

use crate::task::id::TaskID;

use self::term::Terminal;
use alloc::{collections::VecDeque, vec::Vec};
use idos_api::io::termios;

pub struct Console<const COLS: usize, const ROWS: usize> {
    pub terminal: Terminal<COLS, ROWS>,

    /// Stores input that has been entered but not yet flushed to a reader
    pending_input: Vec<u8>,
    /// Stores flushed input. The next read operation on this console will
    /// pull bytes from this.
    pub flushed_input: VecDeque<u8>,

    reader_tasks: Vec<TaskID>,
}

impl<const COLS: usize, const ROWS: usize> Console<COLS, ROWS> {
    pub fn new() -> Self {
        let terminal = Terminal::new();
        Self {
            terminal,
            pending_input: Vec::new(),
            flushed_input: VecDeque::new(),
            reader_tasks: Vec::new(),
        }
    }

    pub fn add_reader_task(&mut self, task_id: TaskID) {
        self.reader_tasks.push(task_id);
    }

    pub fn maybe_terminate_task(&mut self) -> Option<TaskID> {
        if self.reader_tasks.len() > 1 {
            let task = self.reader_tasks.pop();
            if let Some(id) = task {
                crate::task::actions::lifecycle::terminate_task(id, 130);
            }
            return task;
        }
        None
    }

    /// Send bytes of input from the keyboard. If input is flushed, all pending
    /// input will be moved to the flushed input buffer.
    pub fn send_input(&mut self, input: &[u8]) {
        let mut should_flush = false;
        for ch in input {
            match *ch {
                0x00 => continue,
                0x03 => {
                    // Ctrl-C character
                    // check the read mode, in DOS this might just break the read op
                    self.maybe_terminate_task();
                    continue;
                }
                0x08 => {
                    // Backspace character
                    if !self.pending_input.is_empty() {
                        self.terminal.backspace();
                        self.terminal.write_character(0x20); // Write a space to clear the character
                        self.pending_input.pop();
                        self.terminal.backspace(); // Move cursor back again, since writing space moved it forward
                    }
                    continue;
                }
                0x0a => {
                    // Newline character
                    should_flush = true;
                }
                _ => {}
            }
            if self.terminal.lflags & termios::ECHO != 0 {
                self.terminal.write_character(*ch);
            }

            self.pending_input.push(*ch);
        }

        if self.terminal.lflags & termios::ICANON == 0 {
            should_flush = true;
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

    /// Construct an iterator over the TextCells in a specific row of the screen.
    pub fn row_cells_iter(
        &self,
        row: usize,
    ) -> core::iter::Cloned<core::slice::Iter<'_, textmode::TextCell>> {
        let visible = self.terminal.text_buffer.get_visible_buffer();
        let start = row * COLS;
        visible[start..start + COLS].iter().cloned()
    }
}
