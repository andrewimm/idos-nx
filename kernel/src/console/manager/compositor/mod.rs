pub mod cursor;

use alloc::vec::Vec;
use core::marker::ConstParamTy;

use self::cursor::Cursor;
use crate::{
    console::graphics::{font::Font, framebuffer::Framebuffer, Region},
    memory::address::VirtualAddress,
};

use super::ConsoleManager;

#[derive(ConstParamTy, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    Color8Bit,
    Color555,
    Color565,
    Color888,
}

impl ColorDepth {
    pub const fn to_usize(&self) -> usize {
        match self {
            ColorDepth::Color8Bit => 1,
            ColorDepth::Color555 => 2,
            ColorDepth::Color565 => 2,
            ColorDepth::Color888 => 4,
        }
    }
}

struct Window {
    console_index: usize,
}

pub struct Compositor<const COLOR_DEPTH: ColorDepth> {
    /// Framebuffer representing the graphics memory to draw to
    fb: Framebuffer,
    /// Virtual address of the scratch buffer
    scratch_buffer_vaddr: VirtualAddress,
    /// Size of the scratch buffer
    scratch_buffer_size: usize,

    /// If true, on the next render force redraw of all elements
    force_redraw: bool,

    cursor_x: u16,
    cursor_y: u16,
    current_cursor: Cursor,

    dirty_regions: Vec<Region>,

    windows: Vec<Window>,
}

impl<const COLOR_DEPTH: ColorDepth> Compositor<COLOR_DEPTH> {
    pub fn new(fb: Framebuffer) -> Self {
        // allocate double buffer memory
        let scratch_buffer_size =
            (fb.stride as usize) * (fb.height as usize) * COLOR_DEPTH.to_usize();
        let scratch_page_count = (scratch_buffer_size + 0xfff) / 0x1000;
        let scratch_buffer_size = scratch_page_count * 0x1000;
        let scratch_buffer_vaddr = crate::task::actions::memory::map_memory(
            None,
            scratch_buffer_size as u32,
            crate::task::memory::MemoryBacking::Anonymous,
        )
        .unwrap();

        let mut compositor = Self {
            fb,
            scratch_buffer_vaddr,
            scratch_buffer_size,
            force_redraw: true,

            cursor_x: 0,
            cursor_y: 0,
            current_cursor: Cursor::new(16, 16, &DEFAULT_CURSOR),

            dirty_regions: Vec::new(),

            windows: Vec::new(),
        };
        compositor.draw_bg();

        compositor
    }

    pub fn get_scratch_buffer(&mut self) -> &'static mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.scratch_buffer_vaddr.as_ptr_mut::<u8>(),
                self.scratch_buffer_size,
            )
        }
    }

    pub fn draw_bg(&mut self) {
        let buffer = self.get_scratch_buffer();
        match COLOR_DEPTH {
            ColorDepth::Color8Bit => {
                for row in 0..self.fb.height as usize {
                    let offset = self.fb.stride as usize * row;
                    for col in 0..self.fb.width as usize {
                        buffer[offset + col] = 0x00;
                    }
                }
            }
            _ => unimplemented!(),
        }
    }

    pub fn render<F: Font>(
        &mut self,
        mouse_x: u16,
        mouse_y: u16,
        conman: &ConsoleManager,
        font: &F,
    ) {
        self.dirty_regions.clear();

        if self.force_redraw {
            self.dirty_regions.push(Region {
                x: 0,
                y: 0,
                width: self.fb.width,
                height: self.fb.height,
            });
        }

        // draw the top bar

        // draw any active windows
        self.draw_windows(conman, font);

        // copy the dirty region of the scratch buffer to the main framebuffer
        self.blit_scratch_to_fb();

        // The cursor is never drawn to the scratch buffer. Once the scratch
        // buffer is ready and has been copied to the screen, we draw the cursor.
        // This guarantees that the "correct" background behind the cursor is
        // always available in the scratch buffer.
        self.redraw_cursor(mouse_x, mouse_y);

        self.force_redraw = false;
    }

    pub fn add_dirty(&mut self, region: Region) {
        for existing in &self.dirty_regions {
            if existing.fully_contains(&region) {
                return;
            }
        }
        self.dirty_regions.push(region);
    }

    pub fn draw_top_bar(&mut self) {}

    pub fn draw_windows<F: Font>(&mut self, conman: &ConsoleManager, font: &F) {
        let Self {
            ref windows,
            ref mut dirty_regions,
            ..
        } = self;
        windows
            .iter()
            .filter_map(|window| {
                let console_index = window.console_index;
                let mut sub_buffer = Framebuffer {
                    width: self.fb.width - 5,
                    height: self.fb.height - 29,
                    stride: self.fb.stride,
                    buffer: self.scratch_buffer_vaddr + (29 * self.fb.stride as u32) + 5,
                };

                let console = conman.consoles.get(console_index).unwrap();

                conman
                    .draw_window(console, &mut sub_buffer, font)
                    .map(|r| Region {
                        x: r.x + 5,
                        y: r.y + 29,
                        width: r.width,
                        height: r.height,
                    })
            })
            .for_each(|dirty| {
                if !dirty_regions
                    .iter()
                    .any(|existing| existing.fully_contains(&dirty))
                {
                    dirty_regions.push(dirty);
                }
            });
    }

    pub fn blit_scratch_to_fb(&mut self) {
        let fb_raw = self.fb.get_buffer_mut();
        let scratch_raw = self.get_scratch_buffer();

        for region in &self.dirty_regions {
            for row in 0..region.height {
                let offset = (region.y + row) as usize * self.fb.stride as usize
                    + region.x as usize * COLOR_DEPTH.to_usize();
                let length = region.width as usize * COLOR_DEPTH.to_usize();
                fb_raw[offset..(offset + length)]
                    .copy_from_slice(&scratch_raw[offset..(offset + length)]);
            }
        }
    }

    pub fn redraw_cursor(&mut self, new_x: u16, new_y: u16) {
        let cursor_region: Region = Region {
            x: new_x,
            y: new_y,
            width: self.current_cursor.width as u16,
            height: self.current_cursor.height as u16,
        };
        if new_x == self.cursor_x && new_y == self.cursor_y {
            // if the cursor didn't move, check if any dirty region overwrote
            // the cursor
            let mut needs_redraw = false;
            for region in &self.dirty_regions {
                if region.intersects(&cursor_region) {
                    needs_redraw = true;
                    break;
                }
            }
            if !needs_redraw {
                return;
            }
        }

        let fb_raw = self.fb.get_buffer_mut();
        let clear_start = (self.cursor_y as usize * self.fb.stride as usize)
            + (self.cursor_x as usize * COLOR_DEPTH.to_usize());
        let scratch_buffer = self.get_scratch_buffer();
        // TODO: if the cursor changed height, we need to know the *previous* height here
        let clear_rows = self
            .current_cursor
            .height
            .min(self.fb.height - self.cursor_y) as usize;
        let clear_cols = self.current_cursor.width.min(self.fb.width - self.cursor_x) as usize;
        for row in 0..clear_rows {
            let row_offset = clear_start + row * self.fb.stride as usize;
            let copy_size = clear_cols * COLOR_DEPTH.to_usize();
            fb_raw[row_offset..(row_offset + copy_size)]
                .copy_from_slice(&scratch_buffer[row_offset..(row_offset + copy_size)]);
        }

        let total_rows = self.current_cursor.height.min(self.fb.height - new_y) as usize;
        let total_cols = self.current_cursor.width.min(self.fb.width - new_x) as usize;
        let start =
            (new_y as usize * self.fb.stride as usize) + (new_x as usize * COLOR_DEPTH.to_usize());
        match COLOR_DEPTH {
            ColorDepth::Color8Bit => {
                for row in 0..total_rows {
                    let row_offset = start + row * self.fb.stride as usize;
                    let mut cursor_row = self.current_cursor.bitmap[row];
                    for col in 0..total_cols {
                        if cursor_row & 0x8000 != 0 {
                            fb_raw[row_offset + col] = 0x0f;
                        }
                        cursor_row <<= 1;
                    }
                }
            }
            ColorDepth::Color555 => unimplemented!(),
            ColorDepth::Color565 => unimplemented!(),
            ColorDepth::Color888 => unimplemented!(),
        }

        self.cursor_x = new_x;
        self.cursor_y = new_y;
    }

    pub fn add_window(&mut self, console_index: usize) {
        self.windows.push(Window { console_index });
    }
}

static DEFAULT_CURSOR: [u16; 16] = [
    0b1000000000000000,
    0b1100000000000000,
    0b1110000000000000,
    0b1111000000000000,
    0b1111100000000000,
    0b1111110000000000,
    0b1111111000000000,
    0b1111111100000000,
    0b1111111110000000,
    0b1111111111000000,
    0b1111111111100000,
    0b1111111000000000,
    0b1110011000000000,
    0b1100011000000000,
    0b1000001100000000,
    0b0000001100000000,
];
