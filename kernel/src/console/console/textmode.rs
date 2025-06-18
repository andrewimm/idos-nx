//! Structs for representing onscreen text and color as they exist in VGA text
//! mode. This is the internal representation of console text. There are many
//! DOS programs that present a UI by manipulating the text mode buffer directly
//! and we need to support those by mapping a similar buffer at 0x000b_8000.

use crate::memory::address::VirtualAddress;

/// The VGA text buffer is a series of 16-bit values, which contains both an
/// 8-bit character value and an 8-bit value containing both the foreground and
/// background color.
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct TextCell {
    pub glyph: u8,
    pub color: ColorCode,
}

/// The VGA text buffer uses a single 8-bit value for both the foreground and
/// background color. The high 4 bits are the background color and the low 4
/// bits are the foreground color. The colors are defined in the VGA palette
/// as the first 16 values.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ColorCode(pub u8);

impl ColorCode {
    pub const fn new(fg: Color, bg: Color) -> Self {
        Self((bg as u8) << 4 | (fg as u8))
    }

    pub fn set_fg(&mut self, fg: Color) {
        self.0 &= 0xf0;
        self.0 |= fg as u8;
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue,
    Green,
    Cyan,
    Red,
    Magenta,
    Brown,
    LightGray,
    DarkGray,
    LightBlue,
    LightGreen,
    LightCyan,
    LightRed,
    LightMagenta,
    LightBrown,
    White,
}

/// Represents a block of memory that has been allocated to store text console
/// contents. Depending on the size, it may also contain extra leading space for
/// scrollback.
/// The last page of the buffer is able to be mapped directly to 0x000b_8000 for
/// DOS programs. Because this only needs ~4k bytes, there will be a little
/// extra space at the end of the final page that is unused.
pub struct TextBuffer<const COLS: usize, const ROWS: usize> {
    /// starting address of the buffer, from when it was allocated
    buffer_start: VirtualAddress,
    /// total size of the buffer
    buffer_size: usize,
    /// The offset from the start of the buffer to text contents. As text fills
    /// the current screen, past lines will move up and this offset will shrink.
    /// Eventually it will become less than a single row size (but not zero,
    /// since allocated memory is not guaranteed to be a multiple of rows).
    scrollback_start: usize,
}

impl<const COLS: usize, const ROWS: usize> TextBuffer<COLS, ROWS> {
    pub fn new(buffer_start: VirtualAddress, buffer_size: usize) -> Self {
        assert!(buffer_size >= ROWS * COLS * core::mem::size_of::<TextCell>());
        let scrollback_start = buffer_size - 0x1000; // most code here only works if screen size < page size
        Self {
            buffer_start,
            buffer_size,
            scrollback_start,
        }
    }

    pub const fn row_size() -> usize {
        COLS * core::mem::size_of::<TextCell>()
    }

    pub const fn screen_size() -> usize {
        ROWS * COLS * core::mem::size_of::<TextCell>()
    }

    pub const fn unused_tail_size() -> usize {
        0x1000 - (ROWS * COLS * core::mem::size_of::<TextCell>())
    }

    /// Get the entire scrollback buffer. Initially it is the same as the
    /// visible screen area, but successive scroll calls increase its size and
    /// eventually push text up higher into the buffer.
    pub fn get_text_buffer(&self) -> &'static mut [TextCell] {
        let total_size = self.buffer_size - self.scrollback_start - Self::unused_tail_size();
        let buffer_ptr: *mut TextCell =
            (self.buffer_start + self.scrollback_start as u32).as_ptr_mut::<TextCell>();
        unsafe {
            core::slice::from_raw_parts_mut(
                buffer_ptr,
                total_size / core::mem::size_of::<TextCell>(),
            )
        }
    }

    pub fn get_visible_buffer_byte_ptr(&self) -> *mut u8 {
        let offset = self.buffer_size - 0x1000;
        (self.buffer_start + offset as u32).as_ptr_mut::<u8>()
    }

    pub fn get_visible_buffer(&self) -> &'static mut [TextCell] {
        let offset = self.buffer_size - 0x1000;
        let ptr = (self.buffer_start + offset as u32).as_ptr_mut::<TextCell>();
        unsafe { core::slice::from_raw_parts_mut(ptr, ROWS * COLS) }
    }

    pub fn scroll(&mut self) {
        if self.scrollback_start >= Self::row_size() {
            //self.scrollback_start -= Self::row_size();
        }

        let total_buffer = self.get_text_buffer();
        let total_rows = total_buffer.len() / COLS;
        for i in 0..(total_rows - 1) {
            let region_start = i * COLS;
            let region_end = (i + 2) * COLS;
            let copy_region = &mut total_buffer[region_start..region_end];
            let (copy_dest, copy_src) = copy_region.split_at_mut(COLS);
            copy_dest.copy_from_slice(copy_src);
        }
        let final_row_offset = total_buffer.len() - COLS;
        for i in 0..COLS {
            total_buffer[final_row_offset + i] = TextCell {
                glyph: 0x20,
                color: ColorCode::new(Color::LightGray, Color::Black),
            };
        }
    }
}

/// Generate a TextBuffer with the dimensions of a VGA text-mode screen, backed
/// by 2 pages of memory for the scrollback buffer.
pub fn create_text_buffer() -> TextBuffer<80, 25> {
    let alloc_buffer = crate::task::actions::memory::map_memory(
        None,
        0x2000,
        crate::task::memory::MemoryBacking::Anonymous,
    )
    .unwrap();
    TextBuffer::new(alloc_buffer, 0x2000)
}
