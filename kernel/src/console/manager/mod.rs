pub mod compositor;
pub mod decor;
pub mod hit;
pub mod topbar;
pub mod ui;

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

    /// mapping of open handles to the consoles they reference.
    /// Each entry is (console_id, ref_count). The ref count is incremented
    /// on share (duplicate) and decremented on close; the slot is only
    /// removed when the count reaches zero.
    pub open_io: SlotList<(usize, u32)>,
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
        avail_w: u16,
        avail_h: u16,
        force: bool,
        hover_button: Option<u8>,
    ) -> (u16, u16, Option<Region>) {
        let window_pos = Point { x: 0, y: 0 };
        let bpp = (fb.stride / fb.width) as usize;
        let bpp = if bpp == 0 { 1 } else { bpp };

        // In text mode, skip rendering if nothing changed
        if !force && console.terminal.graphics_buffer.is_none() && !console.dirty {
            return (avail_w, avail_h, None);
        }

        let content_y = decor::CONTENT_Y as usize;
        let content_x = decor::CONTENT_X as usize;
        let focused = true; // TODO: track focus per window

        let inner_width = avail_w;
        self::decor::draw_window_bar(fb, window_pos, inner_width, font, "C:\\COMMAND.ELF", focused, bpp, hover_button);

        // The outer area is always avail_w × avail_h (border + black fill).
        // The terminal content is drawn at its natural size within that.
        let outer_w = avail_w as usize;
        let outer_h = avail_h as usize;

        // Fill the entire content area with black
        let buffer = fb.get_buffer_mut();
        for row in 0..outer_h {
            let offset = (content_y + row) * fb.stride as usize + content_x * bpp;
            for px in 0..outer_w {
                crate::console::graphics::write_pixel(buffer, offset + px * bpp, 0x000000, bpp);
            }
        }

        if let Some(graphics_buffer) = &console.terminal.graphics_buffer {
            let gfx_w = (graphics_buffer.width as usize).min(outer_w);
            let gfx_h = (graphics_buffer.height as usize).min(outer_h);

            if !force && !console.dirty && graphics_buffer.read_dirty_rect().is_none() {
                return (avail_w, avail_h, None);
            }
            graphics_buffer.clear_dirty_rect();

            let copy_width = gfx_w.min(graphics_buffer.width as usize);
            let copy_height = gfx_h.min(graphics_buffer.height as usize);
            let raw_buffer = graphics_buffer.get_pixels();
            let src_bpp = (graphics_buffer.bits_per_pixel + 7) / 8;

            for row in 0..copy_height {
                let dest_offset = (content_y + row) * fb.stride as usize + content_x * bpp;
                let src_offset = row * graphics_buffer.width as usize * src_bpp;

                if src_bpp == bpp {
                    let byte_width = copy_width * bpp;
                    buffer[dest_offset..dest_offset + byte_width]
                        .copy_from_slice(&raw_buffer[src_offset..src_offset + byte_width]);
                } else if src_bpp == 1 {
                    let palette = console.terminal.get_palette();
                    for px in 0..copy_width {
                        let color = palette[raw_buffer[src_offset + px] as usize];
                        crate::console::graphics::write_pixel(buffer, dest_offset + px * bpp, color, bpp);
                    }
                }
            }
        } else {
            let palette = console.terminal.get_palette();
            for row in 0..ROWS {
                let colored_chars = console.row_cells_iter(row).map(|cell| {
                    let fg_index = (cell.color.0 & 0x0F) as usize;
                    let bg_index = ((cell.color.0 >> 4) & 0x0F) as usize;
                    (cell.glyph, palette[fg_index], palette[bg_index])
                });
                font.draw_colored_string(
                    fb,
                    window_pos.x + content_x as u16,
                    (window_pos.y + content_y as u16) + (row as u16 * 16),
                    colored_chars,
                    bpp,
                );
            }
        };

        self::decor::draw_window_border(fb, window_pos, avail_w, avail_h, focused, bpp);

        (
            avail_w,
            avail_h,
            Some(Region {
                x: window_pos.x,
                y: window_pos.y,
                width: avail_w + decor::DECOR_EXTRA_W,
                height: avail_h + decor::DECOR_EXTRA_H,
            }),
        )
    }
}
