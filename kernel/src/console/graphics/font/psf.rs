//! PSF (PC Screen Font) files are the format used by the Linux console. It's
//! very straightforward, and similar to how we want to represent Glyphs in
//! memory.

use crate::task::actions::{
    handle::create_file_handle,
    io::{open_sync, read_sync},
};

use super::super::framebuffer::Framebuffer;
use super::{Font, Glyph};
use alloc::vec::Vec;
use idos_api::io::error::IOError;

pub struct PsfFont {
    width: u8,
    height: u8,
    glyphs: Vec<Glyph>,
}

impl PsfFont {
    pub fn from_file(path: &str) -> Result<Self, IOError> {
        let handle = create_file_handle();
        let _ = open_sync(handle, path)?;

        let mut header = [0u8; 4];
        let _ = read_sync(handle, &mut header, 0)?;
        if header[0..2] != [0x36, 0x04] {
            return Err(IOError::NotFound);
        }

        let mode = header[2];
        let height = header[3];
        let glyph_count = if (mode & 1) == 0 { 256 } else { 512 };
        let width = 8; // PSF1 is 8 px wide

        let mut bitmap = Vec::with_capacity(height as usize);
        for _ in 0..height {
            bitmap.push(0);
        }
        let mut glyphs = Vec::with_capacity(glyph_count);
        let mut read_offset = 4;
        for _ in 0..glyph_count {
            let _ = read_sync(handle, bitmap.as_mut_slice(), read_offset);
            let glyph = Glyph::from_bitmap(width, height, bitmap.clone());
            glyphs.push(glyph);
            read_offset += height as u32;
        }

        Ok(Self {
            width,
            height,
            glyphs,
        })
    }
}

impl Font for PsfFont {
    fn get_glyph(&self, byte: u8) -> Option<&Glyph> {
        if (byte as usize) < self.glyphs.len() {
            Some(&self.glyphs[byte as usize])
        } else {
            None
        }
    }

    fn get_height(&self) -> u8 {
        self.height
    }
}
