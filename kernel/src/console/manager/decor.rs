//! Rendering window decoration

use super::super::graphics::font::Font;
use super::super::graphics::framebuffer::Framebuffer;
use super::super::graphics::Point;

const BORDER_WIDTH: usize = 2;
const WINDOW_BAR_HEIGHT: usize = 18;

pub fn draw_window_bar<F: Font>(
    fb: &mut Framebuffer,
    window_pos: Point,
    inner_width: u16,
    font: &F,
    title: &str,
) {
    let tab_width = font.compute_width(title.as_bytes()) as usize + 4;
    let max_width = inner_width as usize + BORDER_WIDTH * 2;
    let mut fb_offset = (window_pos.y * fb.stride + window_pos.x) as usize;
    let framebuffer = fb.get_buffer_mut();

    let width = tab_width.min(max_width);
    for _ in 0..18 {
        for x in 0..width {
            framebuffer[fb_offset + x] = 0x1d;
        }
        fb_offset += fb.stride as usize;
    }

    font.draw_string(fb, window_pos.x + 2, window_pos.y + 2, title.bytes(), 0x00);

    //draw_close_button(fb, window_pos, inner_width, false);
}

const CLOSE_BUTTON_SIZE: usize = 16;
const CLOSE_BUTTON: [u16; CLOSE_BUTTON_SIZE] = [
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1000_0000_0000_0000,
    0b1111_1111_1111_1111,
];

pub fn draw_close_button(fb: &mut Framebuffer, window_pos: Point, inner_width: u16, pressed: bool) {
    let total_width: usize = inner_width as usize + BORDER_WIDTH * 2;
    let button_x = (total_width - CLOSE_BUTTON_SIZE) + window_pos.x as usize;
    let framebuffer = fb.get_buffer_mut();

    let mut fb_offset = (window_pos.y * fb.stride) as usize + button_x;

    for row in 0..CLOSE_BUTTON_SIZE {
        let mut button_row = CLOSE_BUTTON[row];
        for col in 0..CLOSE_BUTTON_SIZE {
            if button_row & 0x8000 != 0 {
                framebuffer[fb_offset + col] = 0x00;
            } else if pressed {
                framebuffer[fb_offset + col] = 0x15;
            }
            button_row <<= 1;
        }
        fb_offset += fb.stride as usize;
    }
}

pub fn draw_window_border(
    fb: &mut Framebuffer,
    window_pos: Point,
    inner_width: u16,
    inner_height: u16,
) {
    let total_width: usize = inner_width as usize + BORDER_WIDTH * 2;
    let mut fb_offset =
        ((window_pos.y + WINDOW_BAR_HEIGHT as u16) * fb.stride + window_pos.x) as usize;
    let framebuffer = fb.get_buffer_mut();

    for y in 0..BORDER_WIDTH {
        for x in 0..total_width {
            framebuffer[fb_offset + x] = 0x1d;
        }
        fb_offset += fb.stride as usize;
    }

    for y in 0..inner_height as usize {
        for x in 0..BORDER_WIDTH {
            framebuffer[fb_offset + x] = 0x1d;
        }
        for x in (total_width - BORDER_WIDTH)..total_width {
            framebuffer[fb_offset + x] = 0x1d;
        }
        fb_offset += fb.stride as usize;
    }

    for y in 0..BORDER_WIDTH {
        for x in 0..total_width {
            framebuffer[fb_offset + x] = 0x1d;
        }
        fb_offset += fb.stride as usize;
    }
}
