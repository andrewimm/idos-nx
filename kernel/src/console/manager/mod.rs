pub mod compositor;
pub mod decor;
pub mod topbar;
pub mod window;

use crate::collections::SlotList;
use crate::io::filesystem::install_task_dev;
use crate::task::id::TaskID;
use crate::task::switching::get_current_id;

use super::driver::PendingRead;
use super::graphics::font::Font;
use super::graphics::framebuffer::Framebuffer;
use super::graphics::{Point, Region};
use super::{
    console::Console,
    input::{KeyAction, KeyState},
};
use alloc::{collections::VecDeque, vec::Vec};

const COLS: usize = 80;
const ROWS: usize = 25;

pub struct ConsoleManager {
    key_state: KeyState,
    pub current_console: usize,
    pub consoles: Vec<Console<COLS, ROWS>>,

    /// mapping of open handles to the consoles they reference
    pub open_io: SlotList<usize>,
    pub pending_reads: SlotList<VecDeque<PendingRead>>,
}

impl ConsoleManager {
    pub fn new() -> Self {
        let consoles = Vec::with_capacity(1);

        Self {
            key_state: KeyState::new(),
            current_console: 0,
            consoles,

            open_io: SlotList::new(),
            pending_reads: SlotList::new(),
        }
    }

    pub fn add_console(&mut self) -> usize {
        let new_console = Console::new();
        // the new memory may be any value; make sure it's all space characters
        new_console.terminal.clear_buffer();
        self.consoles.push(new_console);
        let index = self.consoles.len() - 1;

        // each console needs a device driver installed so that programs like
        // the command prompt can read / write to it
        let name = alloc::format!("CON{}", index + 1);
        install_task_dev(&name, get_current_id(), index as u32);

        index
    }

    pub fn attach_reader_task_to_console(&mut self, console_index: usize, task: TaskID) {
        let console = self.consoles.get_mut(console_index).unwrap();
        console.add_reader_task(task);
    }

    /// Take a key action from the keyboard interrupt handler and send it to the
    /// current console for processing. Depending on the key pressed and the
    /// mode of the console, it may trigger a flush. If any content is flushed,
    /// it will also check for pending reads and copy bytes to them.
    pub fn handle_key_action(&mut self, action: KeyAction) {
        let mut input_bytes: [u8; 4] = [0; 4];
        let result = self.key_state.process_key_action(action, &mut input_bytes);
        if let Some(len) = result {
            // send input buffer to current console
            let input = &input_bytes[0..len];
            let console: &mut Console<COLS, ROWS> =
                self.consoles.get_mut(self.current_console).unwrap();
            console.send_input(input);

            if console.flushed_input.len() > 0 {
                // if any input was flushed, check for pending reads and complete them
                if let Some(queue) = self.pending_reads.get_mut(self.current_console) {
                    while !queue.is_empty() {
                        let pending_read = queue.pop_front().unwrap();
                        pending_read.complete(&mut console.flushed_input);
                    }
                }
            }
        }
    }

    // Move these to another location:

    pub fn draw_window<F: Font>(
        &self,
        console: &Console<COLS, ROWS>,
        fb: &mut Framebuffer,
        font: &F,
    ) -> Option<Region> {
        let window_pos = Point { x: 0, y: 0 };
        self::decor::draw_window_bar(fb, window_pos, 180, font, "C:\\COMMAND.ELF");

        // This needs to be an abstracted call to fill a rectangle, since
        // it cannot assume a color depth
        let buffer = fb.get_buffer_mut();
        for row in 0..400 {
            let offset = (20 + row) * fb.stride as usize + 2;
            for px in 0..640 {
                buffer[offset + px] = 0x00;
            }
        }

        for row in 0..ROWS {
            font.draw_string(
                fb,
                window_pos.x + 2,
                (window_pos.y + 20) + (row as u16 * 16),
                console.row_text_iter(row),
                0x0f,
            );
        }

        self::decor::draw_window_border(fb, window_pos, 640, 400);

        Some(Region {
            x: window_pos.x,
            y: window_pos.y,
            width: 644,
            height: 422,
        })
    }
}
