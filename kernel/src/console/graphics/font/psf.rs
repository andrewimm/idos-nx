//! PSF (PC Screen Font) files are the format used by the Linux console. It's
//! very straightforward, and similar to how we want to represent Glyphs in
//! memory.

use crate::task::actions::{
    handle::create_file_handle,
    io::{close_sync, open_sync, read_sync},
};

use super::{Font, Glyph};
use alloc::vec::Vec;
use idos_api::io::error::IoError;

pub struct PsfFont {
    height: u8,
    glyphs: Vec<Glyph>,
}

impl PsfFont {
    pub fn from_file(path: &str) -> Result<Self, IoError> {
        let handle = create_file_handle();
        let _ = open_sync(handle, path)?;

        let mut header = [0u8; 4];
        let _ = read_sync(handle, &mut header, 0)?;
        if header[0..2] != [0x36, 0x04] {
            return Err(IoError::NotFound);
        }

        let mode = header[2];
        let height = header[3];
        let glyph_count = if (mode & 1) == 0 { 256 } else { 512 };
        let width = 8; // PSF1 is 8 px wide

        const GLYPH_PER_READ: usize = 16;

        let mut bitmap = Vec::with_capacity(GLYPH_PER_READ * height as usize);
        for _ in 0..(height as usize * GLYPH_PER_READ) {
            bitmap.push(0);
        }
        let mut bitmap_offset = 0;
        let mut glyphs = Vec::with_capacity(glyph_count);
        let mut read_offset = 4;
        for _ in 0..glyph_count {
            if bitmap_offset == 0 {
                let _ = read_sync(handle, bitmap.as_mut_slice(), read_offset);
            }
            let slice_offset = bitmap_offset * height as usize;
            let glyph = Glyph::from_slice(
                width,
                height,
                &bitmap[slice_offset..(slice_offset + height as usize)],
            );
            glyphs.push(glyph);
            read_offset += height as u32;
            bitmap_offset += 1;
            bitmap_offset %= GLYPH_PER_READ;
        }
        let _ = close_sync(handle);

        Ok(Self { height, glyphs })
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
