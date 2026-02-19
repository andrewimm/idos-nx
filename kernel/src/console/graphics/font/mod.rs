//! Handling of bitmap fonts

pub mod psf;

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

    pub fn from_bitmap(width: u8, height: u8, bitmap: Vec<u8>) -> Self {
        Self {
            width,
            height,
            bitmap,
        }
    }

    pub fn from_slice(width: u8, height: u8, bitmap: &[u8]) -> Self {
        Self {
            width,
            height,
            bitmap: Vec::from(bitmap),
        }
    }

    pub fn draw_row(&self, framebuffer: &Framebuffer, x: u16, y: u16, row: u8, color: u32, bytes_per_pixel: usize) {
        let draw_offset = (y as usize) * (framebuffer.stride as usize) + (x as usize) * bytes_per_pixel;
        let glyph_stride = (self.width as usize + 7) / 8;
        let mut bitmap_byte_offset = (row as usize) * glyph_stride;
        let mut bitmap_byte = self.bitmap[bitmap_byte_offset];
        let mut shift = 0;
        let raw_buffer = framebuffer.get_buffer_mut();
        for col in 0..self.width as usize {
            if bitmap_byte & 0x80 != 0 {
                super::write_pixel(raw_buffer, draw_offset + col * bytes_per_pixel, color, bytes_per_pixel);
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

pub trait Font {
    fn get_glyph(&self, byte: u8) -> Option<&Glyph>;

    fn get_height(&self) -> u8;

    fn compute_width(&self, byte_string: &[u8]) -> u16 {
        let glyphs = byte_string.iter().filter_map(|byte| self.get_glyph(*byte));
        let mut width = 0;
        for glyph in glyphs {
            width += glyph.width as u16;
        }
        width
    }

    fn draw_string<T: Iterator<Item = u8> + Clone>(
        &self,
        framebuffer: &Framebuffer,
        x: u16,
        y: u16,
        bytes: T,
        color: u32,
        bytes_per_pixel: usize,
    ) {
        let height = self.get_height();
        // assumes all glyphs are the same height...
        for row in 0..height {
            let glyphs = bytes.clone().filter_map(|byte| self.get_glyph(byte));
            let mut run_offset = 0;
            for glyph in glyphs {
                glyph.draw_row(framebuffer, x + run_offset, y + row as u16, row, color, bytes_per_pixel);
                run_offset += glyph.width as u16;
            }
        }
    }

    /// Like draw_string, but each character carries its own color.
    fn draw_colored_string<T: Iterator<Item = (u8, u32)> + Clone>(
        &self,
        framebuffer: &Framebuffer,
        x: u16,
        y: u16,
        chars: T,
        bytes_per_pixel: usize,
    ) {
        let height = self.get_height();
        for row in 0..height {
            let mut run_offset = 0u16;
            for (byte, color) in chars.clone() {
                if let Some(glyph) = self.get_glyph(byte) {
                    glyph.draw_row(framebuffer, x + run_offset, y + row as u16, row, color, bytes_per_pixel);
                    run_offset += glyph.width as u16;
                }
            }
        }
    }
}
