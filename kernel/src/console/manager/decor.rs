//! Rendering window decoration

use super::super::graphics::font::Font;
use super::super::graphics::framebuffer::Framebuffer;
use super::super::graphics::{
    write_pixel, Point, Region, ACCENT, BAR_TEXT, BTN_CLOSE_HOVER_BG, BTN_CLOSE_HOVER_BORDER,
    BTN_HOVER_BG, BTN_HOVER_BORDER, WIN_BORDER, WIN_TITLEBAR, WIN_TITLEBAR_ACTIVE, WIN_TITLE_TEXT,
};

pub const BORDER_WIDTH: usize = 1;
pub const WINDOW_BAR_HEIGHT: usize = 24;

/// Total extra width added by decorations (border left + border right)
pub const DECOR_EXTRA_W: u16 = (BORDER_WIDTH * 2) as u16;
/// Total extra height added by decorations (bar + border top + border bottom)
pub const DECOR_EXTRA_H: u16 = (WINDOW_BAR_HEIGHT + BORDER_WIDTH * 2) as u16;
/// Y offset from window top-left to content area
pub const CONTENT_Y: u16 = (WINDOW_BAR_HEIGHT + BORDER_WIDTH) as u16;
/// X offset from window top-left to content area
pub const CONTENT_X: u16 = BORDER_WIDTH as u16;

pub const BTN_W: usize = 18;
pub const BTN_H: usize = 16;
pub const BTN_GAP: usize = 4;
pub const BTN_COUNT: usize = 3;
pub const BTN_PAD_RIGHT: usize = 8;
pub const BTN_AREA_W: usize = BTN_COUNT * BTN_W + (BTN_COUNT - 1) * BTN_GAP;

pub fn draw_window_bar<F: Font>(
    fb: &mut Framebuffer,
    window_pos: Point,
    inner_width: u16,
    font: &F,
    title: &str,
    focused: bool,
    bytes_per_pixel: usize,
    hover_button: Option<u8>,
) {
    let total_width = inner_width as usize + BORDER_WIDTH * 2;
    let bar_bg = if focused { WIN_TITLEBAR_ACTIVE } else { WIN_TITLEBAR };
    let bpp = bytes_per_pixel;

    let mut fb_offset =
        (window_pos.y as usize * fb.stride as usize) + (window_pos.x as usize * bpp);
    let framebuffer = fb.get_buffer_mut();

    // Fill the entire title bar
    for _ in 0..WINDOW_BAR_HEIGHT {
        for x in 0..total_width {
            write_pixel(framebuffer, fb_offset + x * bpp, bar_bg, bpp);
        }
        fb_offset += fb.stride as usize;
    }

    // Title text, vertically centered, 8px from left
    let text_y = (WINDOW_BAR_HEIGHT as u16 - font.get_height() as u16) / 2;
    font.draw_string(
        fb,
        window_pos.x + 8,
        window_pos.y + text_y,
        title.bytes(),
        WIN_TITLE_TEXT,
        bpp,
    );

    // Three buttons on the right
    let buttons_x = window_pos.x as usize + total_width - BTN_PAD_RIGHT - BTN_AREA_W;
    let buttons_y = window_pos.y as usize + (WINDOW_BAR_HEIGHT - BTN_H) / 2;

    for i in 0..BTN_COUNT {
        let bx = buttons_x + i * (BTN_W + BTN_GAP);
        let is_hovered = hover_button == Some(i as u8);
        if is_hovered {
            let (bg, border) = if i == 2 {
                (BTN_CLOSE_HOVER_BG, BTN_CLOSE_HOVER_BORDER)
            } else {
                (BTN_HOVER_BG, BTN_HOVER_BORDER)
            };
            draw_button_filled(framebuffer, fb.stride as usize, bx, buttons_y, bpp, bg, border);
        } else {
            draw_button_outline(framebuffer, fb.stride as usize, bx, buttons_y, bpp, WIN_BORDER);
        }
    }
}

/// Draw a 1px bordered rectangle (button outline, no fill beyond what's already there)
fn draw_button_outline(
    buffer: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    bpp: usize,
    color: u32,
) {
    // Top edge
    let top_offset = y * stride + x * bpp;
    for col in 0..BTN_W {
        write_pixel(buffer, top_offset + col * bpp, color, bpp);
    }
    // Bottom edge
    let bot_offset = (y + BTN_H - 1) * stride + x * bpp;
    for col in 0..BTN_W {
        write_pixel(buffer, bot_offset + col * bpp, color, bpp);
    }
    // Left and right edges
    for row in 1..(BTN_H - 1) {
        let row_offset = (y + row) * stride + x * bpp;
        write_pixel(buffer, row_offset, color, bpp);
        write_pixel(buffer, row_offset + (BTN_W - 1) * bpp, color, bpp);
    }
}

/// Draw a filled button rectangle with a 1px border
fn draw_button_filled(
    buffer: &mut [u8],
    stride: usize,
    x: usize,
    y: usize,
    bpp: usize,
    bg: u32,
    border: u32,
) {
    for row in 0..BTN_H {
        let row_offset = (y + row) * stride + x * bpp;
        let is_edge_row = row == 0 || row == BTN_H - 1;
        for col in 0..BTN_W {
            let is_edge = is_edge_row || col == 0 || col == BTN_W - 1;
            let color = if is_edge { border } else { bg };
            write_pixel(buffer, row_offset + col * bpp, color, bpp);
        }
    }
}

pub fn draw_window_border(
    fb: &mut Framebuffer,
    window_pos: Point,
    inner_width: u16,
    inner_height: u16,
    focused: bool,
    bytes_per_pixel: usize,
) {
    let border_color = if focused { ACCENT } else { WIN_BORDER };
    let total_width = inner_width as usize + BORDER_WIDTH * 2;
    let bpp = bytes_per_pixel;
    let framebuffer = fb.get_buffer_mut();

    // Border starts below the title bar
    let border_top_y = window_pos.y as usize + WINDOW_BAR_HEIGHT;
    let mut fb_offset = border_top_y * fb.stride as usize + window_pos.x as usize * bpp;

    // Top border
    for _ in 0..BORDER_WIDTH {
        for x in 0..total_width {
            write_pixel(framebuffer, fb_offset + x * bpp, border_color, bpp);
        }
        fb_offset += fb.stride as usize;
    }

    // Left and right borders
    for _ in 0..inner_height as usize {
        for x in 0..BORDER_WIDTH {
            write_pixel(framebuffer, fb_offset + x * bpp, border_color, bpp);
        }
        for x in (total_width - BORDER_WIDTH)..total_width {
            write_pixel(framebuffer, fb_offset + x * bpp, border_color, bpp);
        }
        fb_offset += fb.stride as usize;
    }

    // Bottom border
    for _ in 0..BORDER_WIDTH {
        for x in 0..total_width {
            write_pixel(framebuffer, fb_offset + x * bpp, border_color, bpp);
        }
        fb_offset += fb.stride as usize;
    }
}

/// Return the screen-space rect for a title bar button.
pub fn button_screen_rect(win_x: u16, win_y: u16, inner_width: u16, button_index: usize) -> Region {
    let total_width = inner_width as usize + BORDER_WIDTH * 2;
    let buttons_x = win_x as usize + total_width - BTN_PAD_RIGHT - BTN_AREA_W;
    let buttons_y = win_y as usize + (WINDOW_BAR_HEIGHT - BTN_H) / 2;
    let bx = buttons_x + button_index * (BTN_W + BTN_GAP);
    Region {
        x: bx as u16,
        y: buttons_y as u16,
        width: BTN_W as u16,
        height: BTN_H as u16,
    }
}
