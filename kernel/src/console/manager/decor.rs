//! Rendering window decoration

use super::super::graphics::font::Font;
use super::super::graphics::framebuffer::Framebuffer;
use super::super::graphics::{write_pixel, Point, COLOR_BLACK, COLOR_GRAY, COLOR_DARK_GRAY};

const BORDER_WIDTH: usize = 2;
const WINDOW_BAR_HEIGHT: usize = 18;

pub fn draw_window_bar<F: Font>(
    fb: &mut Framebuffer,
    window_pos: Point,
    inner_width: u16,
    font: &F,
    title: &str,
    bytes_per_pixel: usize,
) {
    let tab_width = font.compute_width(title.as_bytes()) as usize + 4;
    let max_width = inner_width as usize + BORDER_WIDTH * 2;
    let mut fb_offset = (window_pos.y * fb.stride + window_pos.x * bytes_per_pixel as u16) as usize;
    let framebuffer = fb.get_buffer_mut();

    let width = tab_width.min(max_width);
    for _ in 0..18 {
        for x in 0..width {
            write_pixel(framebuffer, fb_offset + x * bytes_per_pixel, COLOR_GRAY, bytes_per_pixel);
        }
        fb_offset += fb.stride as usize;
    }

    font.draw_string(fb, window_pos.x + 2, window_pos.y + 2, title.bytes(), COLOR_BLACK, bytes_per_pixel);
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

pub fn draw_close_button(fb: &mut Framebuffer, window_pos: Point, inner_width: u16, pressed: bool, bytes_per_pixel: usize) {
    let total_width: usize = inner_width as usize + BORDER_WIDTH * 2;
    let button_x = (total_width - CLOSE_BUTTON_SIZE) + window_pos.x as usize;
    let framebuffer = fb.get_buffer_mut();

    let mut fb_offset = (window_pos.y * fb.stride) as usize + button_x * bytes_per_pixel;

    for row in 0..CLOSE_BUTTON_SIZE {
        let mut button_row = CLOSE_BUTTON[row];
        for col in 0..CLOSE_BUTTON_SIZE {
            if button_row & 0x8000 != 0 {
                write_pixel(framebuffer, fb_offset + col * bytes_per_pixel, COLOR_BLACK, bytes_per_pixel);
            } else if pressed {
                write_pixel(framebuffer, fb_offset + col * bytes_per_pixel, COLOR_DARK_GRAY, bytes_per_pixel);
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
    bytes_per_pixel: usize,
) {
    let total_width: usize = inner_width as usize + BORDER_WIDTH * 2;
    let mut fb_offset =
        ((window_pos.y + WINDOW_BAR_HEIGHT as u16) * fb.stride + window_pos.x * bytes_per_pixel as u16) as usize;
    let framebuffer = fb.get_buffer_mut();

    for _ in 0..BORDER_WIDTH {
        for x in 0..total_width {
            write_pixel(framebuffer, fb_offset + x * bytes_per_pixel, COLOR_GRAY, bytes_per_pixel);
        }
        fb_offset += fb.stride as usize;
    }

    for _ in 0..inner_height as usize {
        for x in 0..BORDER_WIDTH {
            write_pixel(framebuffer, fb_offset + x * bytes_per_pixel, COLOR_GRAY, bytes_per_pixel);
        }
        for x in (total_width - BORDER_WIDTH)..total_width {
            write_pixel(framebuffer, fb_offset + x * bytes_per_pixel, COLOR_GRAY, bytes_per_pixel);
        }
        fb_offset += fb.stride as usize;
    }

    for _ in 0..BORDER_WIDTH {
        for x in 0..total_width {
            write_pixel(framebuffer, fb_offset + x * bytes_per_pixel, COLOR_GRAY, bytes_per_pixel);
        }
        fb_offset += fb.stride as usize;
    }
}
