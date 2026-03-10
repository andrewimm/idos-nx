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

    pub fn fg(&self) -> u8 {
        self.0 & 0x0f
    }

    pub fn bg(&self) -> u8 {
        (self.0 >> 4) & 0x0f
    }

    pub fn set_fg(&mut self, fg: u8) {
        self.0 = (self.0 & 0xf0) | (fg & 0x0f);
    }

    pub fn set_bg(&mut self, bg: u8) {
        self.0 = (self.0 & 0x0f) | ((bg & 0x0f) << 4);
    }
}

/// Map ANSI SGR color index (0-7) to VGA palette index.
/// ANSI order: black, red, green, yellow, blue, magenta, cyan, white
/// VGA order:  black, blue, green, cyan, red, magenta, brown, light gray
const ANSI_TO_VGA: [u8; 8] = [
    0,  // ANSI 0 black   -> VGA 0 black
    4,  // ANSI 1 red     -> VGA 4 red
    2,  // ANSI 2 green   -> VGA 2 green
    6,  // ANSI 3 yellow  -> VGA 6 brown
    1,  // ANSI 4 blue    -> VGA 1 blue
    5,  // ANSI 5 magenta -> VGA 5 magenta
    3,  // ANSI 6 cyan    -> VGA 3 cyan
    7,  // ANSI 7 white   -> VGA 7 light gray
];

/// Convert an ANSI SGR color code to a VGA palette index.
/// Normal colors (30-37 fg, 40-47 bg) map to VGA 0-7.
/// Bright colors (90-97 fg, 100-107 bg) map to VGA 8-15.
pub fn ansi_color_to_vga(ansi_index: u8, bright: bool) -> u8 {
    let base = ANSI_TO_VGA[ansi_index as usize & 0x07];
    if bright { base + 8 } else { base }
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

/// Ring buffer holding all terminal rows (scrollback + visible screen).
/// The total capacity is `ROWS + scrollback_extra` rows. The terminal
/// writes into the ring at the cursor position relative to `top_row`,
/// and scrolling simply advances `top_row` — no memmove needed.
///
/// For DOS programs that need a contiguous buffer mapped at 0xB8000,
/// the visible rows can be copied out on demand.
pub struct TextBuffer<const COLS: usize, const ROWS: usize> {
    /// The ring buffer holding all rows.
    buffer: VirtualAddress,
    buffer_alloc: usize,
    /// Total number of row slots in the ring.
    capacity: usize,
    /// Ring index of the first visible row (row 0 of the screen).
    /// The visible screen occupies ring slots top_row..top_row+ROWS.
    top_row: usize,
    /// How many scrollback rows exist above top_row. Starts at 0 and
    /// grows up to `capacity - ROWS` as content scrolls off the top.
    scrollback_count: usize,
}

impl<const COLS: usize, const ROWS: usize> TextBuffer<COLS, ROWS> {
    /// Access the entire ring as a flat cell slice.
    fn ring(&self) -> &'static mut [TextCell] {
        let ptr = self.buffer.as_ptr_mut::<TextCell>();
        unsafe { core::slice::from_raw_parts_mut(ptr, self.capacity * COLS) }
    }

    /// Return a mutable reference to a single row in the ring by ring index.
    fn ring_row_mut(&self, ring_idx: usize) -> &'static mut [TextCell] {
        let ring = self.ring();
        let start = ring_idx * COLS;
        &mut ring[start..start + COLS]
    }

    /// Return an immutable reference to a single row in the ring by ring index.
    fn ring_row(&self, ring_idx: usize) -> &'static [TextCell] {
        let ring = self.ring();
        let start = ring_idx * COLS;
        &ring[start..start + COLS]
    }

    /// Wrap a ring index to stay within capacity.
    fn wrap(&self, idx: usize) -> usize {
        idx % self.capacity
    }

    /// The visible screen as a contiguous mutable slice of ROWS × COLS cells.
    /// The terminal's cursor_y / cursor_x index into this for character
    /// writes and ANSI operations.
    ///
    /// If the visible rows wrap around the end of the ring, the buffer is
    /// rotated so they become contiguous. This rotation is O(n) but only
    /// happens once every `capacity - ROWS` scrolls.
    pub fn get_text_buffer(&mut self) -> &'static mut [TextCell] {
        if self.top_row + ROWS > self.capacity {
            self.rotate_to_zero();
        }
        let ring = self.ring();
        let start = self.top_row * COLS;
        &mut ring[start..start + ROWS * COLS]
    }

    /// Alias for get_text_buffer.
    pub fn get_visible_buffer(&mut self) -> &'static mut [TextCell] {
        self.get_text_buffer()
    }

    /// Rotate the ring buffer so that `top_row` ends up at index 0.
    /// This preserves all content and scrollback ordering.
    fn rotate_to_zero(&mut self) {
        if self.top_row == 0 {
            return;
        }
        let ring = self.ring();
        let total = self.capacity * COLS;
        let mid = self.top_row * COLS;
        ring[..total].rotate_left(mid);
        self.top_row = 0;
    }

    /// Number of scrollback rows currently stored.
    pub fn scrollback_count(&self) -> usize {
        self.scrollback_count
    }

    /// Get a row by virtual index across the combined scrollback + visible
    /// content. Row 0 is the oldest scrollback row; row `scrollback_count`
    /// is the first visible row; row `scrollback_count + ROWS - 1` is the
    /// last visible row.
    pub fn row(&self, virtual_row: usize) -> &[TextCell] {
        let total = self.scrollback_count + ROWS;
        assert!(virtual_row < total);
        // The oldest scrollback row is `scrollback_count` rows before top_row.
        let ring_idx = self.wrap(
            self.top_row + self.capacity - self.scrollback_count + virtual_row
        );
        self.ring_row(ring_idx)
    }

    /// Get a mutable reference to a visible-screen row (0..ROWS).
    pub fn visible_row_mut(&self, screen_row: usize) -> &'static mut [TextCell] {
        assert!(screen_row < ROWS);
        let ring_idx = self.wrap(self.top_row + screen_row);
        self.ring_row_mut(ring_idx)
    }

    /// Allocate a new TextBuffer with the given number of extra scrollback rows.
    pub fn allocate(scrollback_rows: usize) -> Self {
        use crate::task::memory::MemoryBacking;

        let capacity = ROWS + scrollback_rows;
        let total_bytes = capacity * COLS * core::mem::size_of::<TextCell>();
        let alloc_size = (total_bytes + 0xfff) & !0xfff;
        let buffer = crate::task::actions::memory::map_memory(
            None,
            alloc_size as u32,
            MemoryBacking::FreeMemory,
        )
        .unwrap();

        Self {
            buffer,
            buffer_alloc: alloc_size,
            capacity,
            top_row: 0,
            scrollback_count: 0,
        }
    }

    /// Scroll the visible screen up by one row. The row that scrolls off
    /// becomes a scrollback row. The new bottom row is cleared.
    /// This is O(1) — just pointer advancement, no copying.
    pub fn scroll(&mut self) {
        // The row at top_row scrolls off into scrollback
        if self.scrollback_count < self.capacity - ROWS {
            self.scrollback_count += 1;
        }
        // Advance top_row — the old top_row is now the newest scrollback row
        self.top_row = self.wrap(self.top_row + 1);

        // Clear the new bottom row (which is the old top_row slot, now
        // repurposed as the new screen bottom)
        let bottom = self.visible_row_mut(ROWS - 1);
        for cell in bottom.iter_mut() {
            *cell = TextCell {
                glyph: 0x20,
                color: ColorCode::new(Color::LightGray, Color::Black),
            };
        }
    }
}

