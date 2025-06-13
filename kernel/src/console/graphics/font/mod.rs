//! Handling of bitmap fonts
//!

use alloc::vec::Vec;

use super::framebuffer::Framebuffer;

/// Standard format for a bitmap font glyph
/// Stores data as a continuous series of 8-bit numbers.
pub struct Glyph {
    pub width: u8,
    pub height: u8,
    pub bitmap: Vec<u8>,
}

impl Glyph {
    pub fn new(width: u8, height: u8) -> Self {
        Self::with_capacity(width, height, 1)
    }

    pub fn with_capacity(width: u8, height: u8, bitmap_bytes: usize) -> Self {
        Self {
            width,
            height,
            bitmap: Vec::with_capacity(bitmap_bytes),
        }
    }

    pub fn draw_row(&self, framebuffer: &Framebuffer, x: u16, y: u16, row: u8, color: u8) {
        let mut offset = (y as usize) * (framebuffer.stride as usize) + (x as usize);
        let glyph_stride = (self.width as usize + 7) / 8;
        let mut bitmap_byte_offset = (row as usize) * glyph_stride;
        let mut bitmap_byte = self.bitmap[bitmap_byte_offset];
        let mut shift = 0;
        let raw_buffer = framebuffer.get_buffer_mut();
        for col in 0..self.width as usize {
            if bitmap_byte & 0x80 != 0 {
                raw_buffer[offset + col] = color;
            }
            bitmap_byte = bitmap_byte << 1;
            shift += 1;
            if shift >= 8 {
                shift = 0;
                bitmap_byte_offset += 1;
                if bitmap_byte_offset >= self.bitmap.len() {
                    break;
                }
                bitmap_byte = self.bitmap[bitmap_byte_offset];
            }
        }
    }
}

pub fn draw_string(framebuffer: &Framebuffer, x: u16, y: u16, glyphs: &[&Glyph], color: u8) {
    let mut offset = (y as usize) * (framebuffer.stride as usize) + (x as usize);
    let raw_buffer = framebuffer.get_buffer_mut();
    // assumes all glyphs are the same height...
    let height = glyphs[0].height;
    for row in 0..height {
        let mut run_offset = 0;
        for glyph in glyphs {
            glyph.draw_row(framebuffer, x + run_offset, y + row as u16, row, color);
            run_offset += glyph.width as u16;
        }
        offset += framebuffer.stride as usize;
    }
}
